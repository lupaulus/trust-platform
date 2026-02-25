# Ladder Logic Editor

✅ **Functional** - Ladder Logic editor with execution engine

## Overview

Visual Ladder Logic editor for IEC 61131-3 ladder programming within VS Code. Supports both **simulation** and **hardware execution**.

## Features

- ✅ Canvas-based editor using **Konva.js** (vanilla)
- ✅ Drag & drop with snap-to-grid (20px)
- ✅ Zoom with mouse wheel (30%-300%)
- ✅ Ladder elements:
  - Contact (NO/NC - Normally Open/Closed)
  - Coil (NORMAL/SET/RESET/NEGATED)
- ✅ **Ladder execution engine** with PLC-style scan cycle
- ✅ **Simulation mode** - In-memory execution
- ✅ **Hardware mode** - Real I/O via RuntimeClient
- ✅ Visual feedback - Active elements highlighted during execution
- ✅ Rung management (add/remove)
- ✅ Power rails rendering

## Execution Modes

### Simulation Mode (Default)
- Runs entirely in VS Code
- No hardware required
- 100ms scan cycle
- Internal memory simulation
- Perfect for development and testing

### Hardware Mode
- Requires `examples/hardware_8do` backend running
- Controls real hardware (EtherCAT EL2008 or GPIO)
- Connects via Unix socket `/tmp/trust-debug.sock`
- Live I/O visualization

## Hardware Execution

To run ladder programs on real hardware:

1. **Start the hardware backend**:
   ```bash
   cd examples/hardware_8do
   sudo ./start.sh
   ```

2. **Open your ladder program** in VS Code:
   ```bash
   code examples/ladder/ethercat-snake.ladder.json
   ```

3. **Click "Run Hardware"** in the toolbar

See [hardware_8do README](../hardware_8do/README.md) for hardware setup.

## Architecture

```
editors/vscode/src/ladder/
├── ladderEngine.types.ts    # Type definitions
├── ladderEngine.ts          # Execution engine (PLC scan cycle)
├── ladderEditor.ts          # VS Code provider
├── webview/
│   ├── LadderEditor.tsx     # Main canvas component (Konva)
│   ├── Toolbar.tsx          # Tools & controls
│   ├── main.tsx             # Entry point
│   └── styles.css           # Styling
```

## Example Programs

### Simulation Examples
- `simple-start-stop.ladder.json` - Basic 2-rung start/stop logic
- `ethercat-snake.ladder.json` - **Knight Rider effect** - 8 LEDs chase pattern using SET/RESET coils

### Hardware Execution

To run ladder programs on **real hardware** (Beckhoff EL2008 or Raspberry Pi GPIO):

1. **Start the hardware backend:**
```bash
cd examples/hardware_8do
sudo ./start.sh
```
This starts trust-runtime with EtherCAT/GPIO drivers and exposes `/tmp/trust-debug.sock`

2. **Open your ladder program** in VS Code:
```bash
code examples/ladder/ethercat-snake.ladder.json
```

3. **Click "Run Hardware"** button in the toolbar
   - Ladder editor connects to `/tmp/trust-debug.sock`
   - Every scan cycle (100ms) sends I/O writes to hardware
   - Watch your LEDs light up in sequence!

See [hardware_8do/README.md](../hardware_8do/README.md) for hardware setup details.

## Ladder Interpreter

The ladder engine implements a traditional PLC scan cycle:

1. **Read Inputs** - From hardware (EtherCAT/GPIO) or simulation
2. **Evaluate Rungs** - Top to bottom, left to right
3. **Write Outputs** - To hardware or simulation memory

**Memory Areas:**
- `%IX` - Digital inputs  
- `%QX` - Digital outputs
- `%MX` - Internal markers (flags)
- `%MW` - Memory words (integers)

**Scan cycle:** 100ms (configurable)

## Tech Stack

- **Konva.js** 9.3.6 - Canvas rendering (vanilla, no react-konva)
- **React** 19.2.4 - UI framework
- **TypeScript** 5.0 - Type safety
- **esbuild** - Fast bundling (IIFE format for VS Code webviews)

## Building

```bash
cd editors/vscode
npm install
npm run build:ladder
```

Output: `media/ladderWebview.js` (386KB IIFE bundle)

## Roadmap

### Phase 1: Core Editor ✅ COMPLETED
- [x] Canvas rendering with Konva
- [x] Contact & Coil elements
- [x] Drag & drop with snap-to-grid
- [x] Zoom and pan
- [x] Large scrollable canvas
- [ ] Element connections visualization (wires)
- [ ] Element properties panel
- [ ] Delete/edit elements

### Phase 2: Execution ✅ COMPLETED
- [x] Ladder interpreter with PLC scan cycle
- [x] Contact evaluation (NO/NC)
- [x] Coil execution (NORMAL/SET/RESET/NEGATED)
- [x] RuntimeClient integration
- [x] Hardware execution via `hardware_8do` backend
- [x] Real-time element highlighting

### Phase 3: More Elements
- [ ] Timer blocks (TON/TOF/TP)
- [ ] Counter blocks (CTU/CTD/CTUD)
- [ ] Comparison blocks (GT/LT/EQ)
- [ ] Math blocks (ADD/SUB/MUL/DIV)
- [ ] Parallel branches
- [ ] Series connections

### Phase 4: Professional Features
- [ ] Auto-routing connections
- [ ] Undo/redo
- [ ] Copy/paste rungs
- [ ] Search/revanilla over react-konva?**
- React 19 compatibility issues with react-konva
- Direct Konva API gives more control
- Smaller bundle size (386KB)
- Better performance

**Why Fabric.js not chosen?**
- Konva has better TypeScript support
- More active development
- Cleaner event handling

**Why interpreted execution instead of ST generation?**
- Same pattern as Statechart/Blockly editors
- Real-time execution visibility
- Simpler debugging
- Direct hardware control via RuntimeClient

## Contributing

This is a prototype. Feedback welcome!

## License

MIT OR Apache-2.0 (same as main project)
