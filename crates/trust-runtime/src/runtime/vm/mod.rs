use std::collections::{HashMap, HashSet};

use smol_str::SmolStr;

use crate::bytecode::{
    BytecodeModule, ConstEntry, ConstPool, PouKind, RefEntry, RefLocation, RefTable, SectionData,
    SectionId, StringTable, TypeData, TypeTable,
};
use crate::error::RuntimeError;
use crate::memory::IoArea;
use crate::task::ProgramDef;
use crate::value::{
    DateTimeValue, DateValue, Duration, LDateTimeValue, LDateValue, LTimeOfDayValue,
    RefSegment as ValueRefSegment, TimeOfDayValue, Value, ValueRef,
};

mod call;
mod debug_map;
mod dispatch;
mod errors;
mod frames;
mod stack;

// VM module ownership notes (Phase B):
// - dispatch: instruction pointer loop + opcode dispatch + storage access bridge.
// - stack: operand stack invariants and overflow/underflow enforcement.
// - frames/call: call-stack and frame lifecycle.
// - errors: VM trap taxonomy and stable RuntimeError mapping.
// - debug_map: symbol/source lookup tables for external name/debug APIs.

use super::core::Runtime;

pub(super) const DEFAULT_INSTRUCTION_BUDGET: usize = 1_000_000;

pub(super) fn execute_program(
    runtime: &mut Runtime,
    program: &ProgramDef,
) -> Result<(), RuntimeError> {
    dispatch::execute_program(runtime, program)
}

pub(super) fn execute_function_block_ref(
    runtime: &mut Runtime,
    reference: &ValueRef,
) -> Result<(), RuntimeError> {
    dispatch::execute_function_block_ref(runtime, reference)
}

#[derive(Debug, Clone)]
pub(super) struct VmModule {
    pub(super) code: Vec<u8>,
    pub(super) strings: Vec<SmolStr>,
    pub(super) refs: Vec<VmRef>,
    pub(super) consts: Vec<Value>,
    pub(super) pou_by_id: HashMap<u32, VmPouEntry>,
    pub(super) program_ids: HashMap<SmolStr, u32>,
    pub(super) function_block_ids: HashMap<SmolStr, u32>,
    #[allow(dead_code)]
    // Populated in Phase B, consumed by debug/event parity work in later phases.
    debug_map: debug_map::VmDebugMap,
    pub(super) instruction_budget: usize,
}

impl VmModule {
    pub(super) fn from_bytecode(module: &BytecodeModule) -> Result<Self, RuntimeError> {
        let strings = match module.section(SectionId::StringTable) {
            Some(SectionData::StringTable(table)) => table,
            _ => return Err(invalid_bytecode("missing STRING_TABLE")),
        };
        let types = match module.section(SectionId::TypeTable) {
            Some(SectionData::TypeTable(table)) => table,
            _ => return Err(invalid_bytecode("missing TYPE_TABLE")),
        };
        let const_pool = match module.section(SectionId::ConstPool) {
            Some(SectionData::ConstPool(table)) => table,
            _ => return Err(invalid_bytecode("missing CONST_POOL")),
        };
        let ref_table = match module.section(SectionId::RefTable) {
            Some(SectionData::RefTable(table)) => table,
            _ => return Err(invalid_bytecode("missing REF_TABLE")),
        };
        let pou_index = match module.section(SectionId::PouIndex) {
            Some(SectionData::PouIndex(index)) => index,
            _ => return Err(invalid_bytecode("missing POU_INDEX")),
        };
        let bodies = match module.section(SectionId::PouBodies) {
            Some(SectionData::PouBodies(code)) => code,
            _ => return Err(invalid_bytecode("missing POU_BODIES")),
        };

        let refs = decode_ref_table(ref_table, strings)?;
        let consts = decode_const_pool_entries(const_pool, types)?;

        let debug_map = debug_map::VmDebugMap::from_sections(
            strings,
            match module.section(SectionId::VarMeta) {
                Some(SectionData::VarMeta(meta)) => Some(meta),
                _ => None,
            },
            match module.section(SectionId::DebugStringTable) {
                Some(SectionData::DebugStringTable(table)) => Some(table),
                _ => None,
            },
            match module.section(SectionId::DebugMap) {
                Some(SectionData::DebugMap(map)) => Some(map),
                _ => None,
            },
        );

        let mut pou_by_id = HashMap::new();
        let mut program_ids = HashMap::new();
        let mut function_block_ids = HashMap::new();

        for entry in &pou_index.entries {
            let name = strings
                .entries
                .get(entry.name_idx as usize)
                .cloned()
                .ok_or_else(|| {
                    invalid_bytecode(format!("invalid POU name string index {}", entry.name_idx))
                })?;
            let code_start = entry.code_offset as usize;
            let code_end = code_start + entry.code_length as usize;
            if code_end > bodies.len() {
                return Err(invalid_bytecode(format!(
                    "POU '{}' code range out of bounds",
                    name
                )));
            }
            let mut vm_entry = VmPouEntry {
                code_start,
                code_end,
                local_ref_start: entry.local_ref_start,
                local_ref_count: entry.local_ref_count,
                primary_instance_owner: None,
            };
            vm_entry.primary_instance_owner =
                infer_primary_instance_owner(&vm_entry, bodies, &refs);
            pou_by_id.insert(entry.id, vm_entry);

            let key = SmolStr::new(name.to_ascii_uppercase());
            if matches!(entry.kind, PouKind::Program) {
                program_ids.insert(key, entry.id);
            } else if matches!(entry.kind, PouKind::FunctionBlock) {
                function_block_ids.insert(key, entry.id);
            }
        }

        Ok(Self {
            code: bodies.clone(),
            strings: strings.entries.clone(),
            refs,
            consts,
            pou_by_id,
            program_ids,
            function_block_ids,
            debug_map,
            instruction_budget: DEFAULT_INSTRUCTION_BUDGET,
        })
    }

    pub(super) fn pou(&self, id: u32) -> Option<&VmPouEntry> {
        self.pou_by_id.get(&id)
    }
}

#[derive(Debug, Clone)]
pub(super) struct VmPouEntry {
    pub(super) code_start: usize,
    pub(super) code_end: usize,
    pub(super) local_ref_start: u32,
    pub(super) local_ref_count: u32,
    pub(super) primary_instance_owner: Option<u32>,
}

#[derive(Debug, Clone)]
pub(super) enum VmRef {
    Global {
        offset: usize,
        path: Vec<ValueRefSegment>,
    },
    Local {
        owner_frame_id: u32,
        offset: usize,
        path: Vec<ValueRefSegment>,
    },
    Instance {
        owner_instance_id: u32,
        offset: usize,
        path: Vec<ValueRefSegment>,
    },
    Retain {
        offset: usize,
        path: Vec<ValueRefSegment>,
    },
    Io {
        area: IoArea,
        offset: usize,
        path: Vec<ValueRefSegment>,
    },
}

pub(super) fn opcode_operand_len(opcode: u8) -> Option<usize> {
    match opcode {
        0x00
        | 0x01
        | 0x06
        | 0x11
        | 0x12
        | 0x13
        | 0x14
        | 0x15
        | 0x23
        | 0x24
        | 0x31
        | 0x32
        | 0x33
        | 0x40..=0x4E
        | 0x50..=0x55 => Some(0),
        0x02..=0x05 | 0x07 | 0x10 | 0x20..=0x22 | 0x30 | 0x60 | 0x70 => Some(4),
        0x08 => Some(8),
        0x09 => Some(12),
        0x16 => Some(1),
        _ => None,
    }
}

fn invalid_bytecode(message: impl Into<SmolStr>) -> RuntimeError {
    RuntimeError::InvalidBytecode(message.into())
}

fn decode_ref_table(
    ref_table: &RefTable,
    strings: &StringTable,
) -> Result<Vec<VmRef>, RuntimeError> {
    let mut refs = Vec::with_capacity(ref_table.entries.len());
    for entry in &ref_table.entries {
        refs.push(decode_vm_ref(entry, strings)?);
    }
    Ok(refs)
}

fn decode_vm_ref(entry: &RefEntry, strings: &StringTable) -> Result<VmRef, RuntimeError> {
    let mut path = Vec::with_capacity(entry.segments.len());
    for segment in &entry.segments {
        match segment {
            crate::bytecode::RefSegment::Index(indices) => {
                path.push(ValueRefSegment::Index(indices.clone()));
            }
            crate::bytecode::RefSegment::Field { name_idx } => {
                let name = strings
                    .entries
                    .get(*name_idx as usize)
                    .cloned()
                    .ok_or_else(|| {
                        invalid_bytecode(format!("invalid ref field string index {name_idx}"))
                    })?;
                path.push(ValueRefSegment::Field(name));
            }
        }
    }

    let offset = entry.offset as usize;
    match entry.location {
        RefLocation::Global => Ok(VmRef::Global { offset, path }),
        RefLocation::Local => Ok(VmRef::Local {
            owner_frame_id: entry.owner_id,
            offset,
            path,
        }),
        RefLocation::Instance => Ok(VmRef::Instance {
            owner_instance_id: entry.owner_id,
            offset,
            path,
        }),
        RefLocation::Retain => Ok(VmRef::Retain { offset, path }),
        RefLocation::Io => {
            let area = match entry.owner_id {
                0 => IoArea::Input,
                1 => IoArea::Output,
                2 => IoArea::Memory,
                other => {
                    return Err(invalid_bytecode(format!(
                        "invalid VM IO owner area {other}"
                    )));
                }
            };
            Ok(VmRef::Io { area, offset, path })
        }
    }
}

fn infer_primary_instance_owner(entry: &VmPouEntry, code: &[u8], refs: &[VmRef]) -> Option<u32> {
    let mut owners = HashSet::new();
    let mut pc = entry.code_start;
    while pc < entry.code_end {
        let opcode = *code.get(pc)?;
        pc += 1;
        let operand_len = opcode_operand_len(opcode)?;
        if pc + operand_len > entry.code_end {
            return None;
        }
        if matches!(opcode, 0x20..=0x22) && operand_len == 4 {
            let bytes = [code[pc], code[pc + 1], code[pc + 2], code[pc + 3]];
            let ref_idx = u32::from_le_bytes(bytes);
            if let Some(VmRef::Instance {
                owner_instance_id, ..
            }) = refs.get(ref_idx as usize)
            {
                owners.insert(*owner_instance_id);
            }
        }
        pc += operand_len;
    }

    if owners.len() == 1 {
        owners.iter().copied().next()
    } else {
        None
    }
}

fn decode_const_pool_entries(
    const_pool: &ConstPool,
    types: &TypeTable,
) -> Result<Vec<Value>, RuntimeError> {
    let mut out = Vec::with_capacity(const_pool.entries.len());
    for entry in &const_pool.entries {
        out.push(decode_const_value(entry, types)?);
    }
    Ok(out)
}

enum ConstKind {
    Primitive(u16),
    Enum,
}

fn resolve_const_kind(
    types: &TypeTable,
    type_id: u32,
    depth: u8,
) -> Result<ConstKind, RuntimeError> {
    if depth > 32 {
        return Err(invalid_bytecode("const type recursion overflow"));
    }
    let entry = types
        .entries
        .get(type_id as usize)
        .ok_or_else(|| invalid_bytecode(format!("invalid const type index {type_id}")))?;
    match &entry.data {
        TypeData::Primitive { prim_id, .. } => Ok(ConstKind::Primitive(*prim_id)),
        TypeData::Alias { target_type_id } => resolve_const_kind(types, *target_type_id, depth + 1),
        TypeData::Subrange { base_type_id, .. } => {
            resolve_const_kind(types, *base_type_id, depth + 1)
        }
        TypeData::Enum { .. } => Ok(ConstKind::Enum),
        _ => Err(invalid_bytecode(format!(
            "unsupported const type kind at index {type_id}"
        ))),
    }
}

fn decode_const_value(entry: &ConstEntry, types: &TypeTable) -> Result<Value, RuntimeError> {
    match resolve_const_kind(types, entry.type_id, 0)? {
        ConstKind::Enum => {
            let bytes = read_exact::<8>(&entry.payload, "enum const payload")?;
            Ok(Value::LInt(i64::from_le_bytes(bytes)))
        }
        ConstKind::Primitive(prim_id) => decode_primitive_constant(prim_id, &entry.payload),
    }
}

fn decode_primitive_constant(prim_id: u16, payload: &[u8]) -> Result<Value, RuntimeError> {
    match prim_id {
        1 => {
            let value = read_exact::<1>(payload, "BOOL const payload")?[0];
            Ok(Value::Bool(value != 0))
        }
        2 => Ok(Value::Byte(
            read_exact::<1>(payload, "BYTE const payload")?[0],
        )),
        3 => Ok(Value::Word(u16::from_le_bytes(read_exact::<2>(
            payload,
            "WORD const payload",
        )?))),
        4 => Ok(Value::DWord(u32::from_le_bytes(read_exact::<4>(
            payload,
            "DWORD const payload",
        )?))),
        5 => Ok(Value::LWord(u64::from_le_bytes(read_exact::<8>(
            payload,
            "LWORD const payload",
        )?))),
        6 => Ok(Value::SInt(i8::from_le_bytes(read_exact::<1>(
            payload,
            "SINT const payload",
        )?))),
        7 => Ok(Value::Int(i16::from_le_bytes(read_exact::<2>(
            payload,
            "INT const payload",
        )?))),
        8 => Ok(Value::DInt(i32::from_le_bytes(read_exact::<4>(
            payload,
            "DINT const payload",
        )?))),
        9 => Ok(Value::LInt(i64::from_le_bytes(read_exact::<8>(
            payload,
            "LINT const payload",
        )?))),
        10 => Ok(Value::USInt(
            read_exact::<1>(payload, "USINT const payload")?[0],
        )),
        11 => Ok(Value::UInt(u16::from_le_bytes(read_exact::<2>(
            payload,
            "UINT const payload",
        )?))),
        12 => Ok(Value::UDInt(u32::from_le_bytes(read_exact::<4>(
            payload,
            "UDINT const payload",
        )?))),
        13 => Ok(Value::ULInt(u64::from_le_bytes(read_exact::<8>(
            payload,
            "ULINT const payload",
        )?))),
        14 => Ok(Value::Real(f32::from_le_bytes(read_exact::<4>(
            payload,
            "REAL const payload",
        )?))),
        15 => Ok(Value::LReal(f64::from_le_bytes(read_exact::<8>(
            payload,
            "LREAL const payload",
        )?))),
        16 => Ok(Value::Time(Duration::from_nanos(i64::from_le_bytes(
            read_exact::<8>(payload, "TIME const payload")?,
        )))),
        17 => Ok(Value::LTime(Duration::from_nanos(i64::from_le_bytes(
            read_exact::<8>(payload, "LTIME const payload")?,
        )))),
        18 => Ok(Value::Date(DateValue::new(i64::from_le_bytes(
            read_exact::<8>(payload, "DATE const payload")?,
        )))),
        19 => Ok(Value::LDate(LDateValue::new(i64::from_le_bytes(
            read_exact::<8>(payload, "LDATE const payload")?,
        )))),
        20 => Ok(Value::Tod(TimeOfDayValue::new(i64::from_le_bytes(
            read_exact::<8>(payload, "TOD const payload")?,
        )))),
        21 => Ok(Value::LTod(LTimeOfDayValue::new(i64::from_le_bytes(
            read_exact::<8>(payload, "LTOD const payload")?,
        )))),
        22 => Ok(Value::Dt(DateTimeValue::new(i64::from_le_bytes(
            read_exact::<8>(payload, "DT const payload")?,
        )))),
        23 => Ok(Value::Ldt(LDateTimeValue::new(i64::from_le_bytes(
            read_exact::<8>(payload, "LDT const payload")?,
        )))),
        24 => {
            let text = std::str::from_utf8(payload)
                .map_err(|err| invalid_bytecode(format!("invalid STRING const UTF-8: {err}")))?;
            Ok(Value::String(SmolStr::new(text)))
        }
        25 => {
            if payload.len() % 2 != 0 {
                return Err(invalid_bytecode("invalid WSTRING const payload length"));
            }
            let units = payload
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            let text = String::from_utf16(&units)
                .map_err(|err| invalid_bytecode(format!("invalid WSTRING const UTF-16: {err}")))?;
            Ok(Value::WString(text))
        }
        26 => Ok(Value::Char(
            read_exact::<1>(payload, "CHAR const payload")?[0],
        )),
        27 => Ok(Value::WChar(u16::from_le_bytes(read_exact::<2>(
            payload,
            "WCHAR const payload",
        )?))),
        other => Err(invalid_bytecode(format!(
            "unsupported const primitive id {other}"
        ))),
    }
}

fn read_exact<const N: usize>(payload: &[u8], kind: &str) -> Result<[u8; N], RuntimeError> {
    if payload.len() != N {
        return Err(invalid_bytecode(format!(
            "invalid {kind} length {}, expected {N}",
            payload.len()
        )));
    }
    let mut out = [0_u8; N];
    out.copy_from_slice(payload);
    Ok(out)
}
