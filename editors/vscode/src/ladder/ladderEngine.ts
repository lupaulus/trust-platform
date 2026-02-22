/**
 * Ladder Logic Engine - Executes ladder programs with PLC-style scan cycle
 * Supports both simulation (mock) and hardware (real I/O) execution modes
 */

import type { RuntimeClient } from "../statechart/runtimeClient";
import type { LadderProgram, LadderElement, Rung } from "./ladderEngine.types";

type ExecutionMode = "simulation" | "hardware";

interface LadderEngineOptions {
  scanCycleMs?: number; // PLC scan cycle duration (default: 100ms)
  runtimeClient?: RuntimeClient;
}

/**
 * Ladder Logic execution engine
 * Implements traditional PLC scan cycle:
 * 1. Read inputs
 * 2. Evaluate logic (top to bottom)
 * 3. Write outputs
 */
export class LadderEngine {
  private program: LadderProgram;
  private mode: ExecutionMode;
  private scanCycleMs: number;
  private runtimeClient?: RuntimeClient;
  
  // Memory areas (simulation mode)
  private inputs: Map<string, boolean> = new Map(); // %IX addresses
  private outputs: Map<string, boolean> = new Map(); // %QX addresses
  private markers: Map<string, boolean> = new Map(); // %MX addresses (internal flags)
  private memoryWords: Map<string, number> = new Map(); // %MW addresses (integers)
  
  // Write buffer (prevents cascade execution in same scan)
  private pendingWrites: Map<string, boolean | number> = new Map();
  
  // Execution control
  private scanIntervalHandle?: NodeJS.Timeout;
  private isRunning = false;
  private scanCount = 0;
  private forcedAddresses: Set<string> = new Set();
  
  // State notification callback
  private onStateChange?: (state: any) => void;

  constructor(
    program: LadderProgram,
    mode: ExecutionMode = "simulation",
    options: LadderEngineOptions = {}
  ) {
    this.program = program;
    this.mode = mode;
    this.scanCycleMs = options.scanCycleMs ?? 100; // Default 100ms scan cycle
    this.runtimeClient = options.runtimeClient;

    // Initialize variables from program
    this.initializeVariables();
  }

  /**
   * Initialize all variables to their default values
   */
  private initializeVariables(): void {
    for (const variable of this.program.variables) {
      const address = variable.address;
      
      if (!address) continue; // Skip if address is undefined
      
      if (address.startsWith("%IX")) {
        this.inputs.set(address, false);
      } else if (address.startsWith("%QX")) {
        this.outputs.set(address, false);
      } else if (address.startsWith("%MX")) {
        this.markers.set(address, false);
      } else if (address.startsWith("%MW")) {
        this.memoryWords.set(address, 0);
      }
    }

    console.log(`🎯 Ladder Engine initialized in ${this.mode} mode`);
    console.log(`   Inputs: ${this.inputs.size}, Outputs: ${this.outputs.size}, Markers: ${this.markers.size}, Words: ${this.memoryWords.size}`);
  }

  /**
   * Start the ladder execution (begin scan cycles)
   */
  async start(): Promise<void> {
    if (this.isRunning) {
      console.warn("⚠️ Ladder engine already running");
      return;
    }

    this.isRunning = true;
    this.scanCount = 0;

    // Set initial trigger for snake pattern
    this.markers.set("%MX0.0", true); // Start trigger

    // Initial scan
    await this.executeScanCycle();

    // Start periodic scan cycles
    this.scanIntervalHandle = setInterval(async () => {
      await this.executeScanCycle();
    }, this.scanCycleMs);

    console.log(`▶️ Ladder execution started (scan cycle: ${this.scanCycleMs}ms)`);
  }

  /**
   * Stop the ladder execution
   */
  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;

    if (this.scanIntervalHandle) {
      clearInterval(this.scanIntervalHandle);
      this.scanIntervalHandle = undefined;
    }

    // Clear all outputs
    await this.clearAllOutputs();

    console.log(`⏹️ Ladder execution stopped (${this.scanCount} scan cycles)`);
  }

  /**
   * Execute one complete PLC scan cycle
   */
  private async executeScanCycle(): Promise<void> {
    this.scanCount++;

    try {
      // Clear pending writes from previous scan
      this.pendingWrites.clear();

      // PHASE 1: Read inputs (from hardware if in hardware mode)
      await this.readInputs();

      // PHASE 2: Evaluate ladder logic (top to bottom)
      await this.evaluateRungs();

      // PHASE 3: Apply pending writes (commit buffered changes)
      this.applyPendingWrites();

      // PHASE 4: Write outputs (to hardware if in hardware mode)
      await this.writeOutputs();

      // Notify state change
      if (this.onStateChange) {
        this.onStateChange(this.getExecutionState());
      }

    } catch (error) {
      console.error(`❌ Error in scan cycle ${this.scanCount}:`, error);
    }
  }

  /**
   * Read all inputs (from hardware in hardware mode)
   */
  private async readInputs(): Promise<void> {
    if (this.mode === "hardware" && this.runtimeClient?.isConnected()) {
      // Read inputs from RuntimeClient
      for (const address of this.inputs.keys()) {
        try {
          const value = await this.runtimeClient.readIo(address);
          this.inputs.set(address, Boolean(value));
        } catch (error) {
          console.error(`❌ Failed to read ${address}:`, error);
        }
      }
    }
    // In simulation mode, inputs are set externally via setInput()
  }

  /**
   * Evaluate all rungs in order (top to bottom)
   */
  private async evaluateRungs(): Promise<void> {
    for (const rung of this.program.rungs) {
      await this.evaluateRung(rung);
    }
  }

  /**
   * Evaluate a single rung
   * A rung is TRUE if all its contact logic evaluates to TRUE
   */
  private async evaluateRung(rung: Rung): Promise<void> {
    // Simplified logic: evaluate each element left to right
    // In a real implementation, you'd parse the connection topology
    
    let rungPower = true; // Start with power available
    const coils: LadderElement[] = [];

    // PHASE 1: Evaluate contacts (determine if rung has power)
    for (const element of rung.elements) {
      if (element.type === "contact") {
        const contactState = this.evaluateContact(element);
        rungPower = rungPower && contactState;
      } else if (element.type === "coil") {
        coils.push(element);
      }
    }

    // PHASE 2: Execute coils with the rung power state
    for (const coil of coils) {
      await this.executeCoil(coil, rungPower);
    }
  }

  /**
   * Evaluate a contact element (NO or NC)
   */
  private evaluateContact(element: LadderElement): boolean {
    const contact = element as any; // Type assertion for runtime access
    const value = this.readVariable(contact.variable);
    
    if (contact.contactType === "NO") {
      // Normally Open: TRUE if variable is TRUE
      return value;
    } else if (contact.contactType === "NC") {
      // Normally Closed: TRUE if variable is FALSE
      return !value;
    }
    
    return false;
  }

  /**
   * Execute a coil element (NORMAL, SET, RESET, NEGATED)
   * Writes go to buffer, not directly to memory (prevents cascade in same scan)
   */
  private async executeCoil(element: LadderElement, rungPower: boolean): Promise<void> {
    const coil = element as any; // Type assertion for runtime access
    
    switch (coil.coilType) {
      case "NORMAL":
        // Standard coil: output = rung power
        this.bufferWrite(coil.variable, rungPower);
        break;

      case "SET":
        // Set coil: if rung is TRUE, latch output to TRUE (stays TRUE until RESET)
        if (rungPower) {
          this.bufferWrite(coil.variable, true);
        }
        break;

      case "RESET":
        // Reset coil: if rung is TRUE, unlatch output to FALSE
        if (rungPower) {
          this.bufferWrite(coil.variable, false);
        }
        break;

      case "NEGATED":
        // Negated coil: output = NOT rung power
        this.bufferWrite(coil.variable, !rungPower);
        break;
    }
  }

  /**
   * Buffer a write operation (doesn't change memory until applyPendingWrites)
   */
  private bufferWrite(address: string, value: boolean | number): void {
    this.pendingWrites.set(address, value);
  }

  /**
   * Apply all buffered writes to actual memory
   * This ensures writes only take effect at end of scan, preventing cascade
   */
  private applyPendingWrites(): void {
    for (const [address, value] of this.pendingWrites.entries()) {
      if (typeof value === 'boolean') {
        if (address.startsWith("%IX")) {
          this.inputs.set(address, value);
        } else if (address.startsWith("%QX")) {
          this.outputs.set(address, value);
        } else if (address.startsWith("%MX")) {
          this.markers.set(address, value);
        }
      } else if (typeof value === 'number') {
        if (address.startsWith("%MW")) {
          this.memoryWords.set(address, value);
        }
      }
    }
  }

  /**
   * Write a variable directly (use only for initialization, not during scan)
   */

  /**
   * Read a variable value (supports %IX, %QX, %MX, %MW)
   */
  private readVariable(address: string): boolean {
    if (address.startsWith("%IX")) {
      return this.inputs.get(address) ?? false;
    } else if (address.startsWith("%QX")) {
      return this.outputs.get(address) ?? false;
    } else if (address.startsWith("%MX")) {
      return this.markers.get(address) ?? false;
    } else if (address.startsWith("%MW")) {
      // For boolean context, check if word != 0
      return (this.memoryWords.get(address) ?? 0) !== 0;
    }
    
    console.warn(`⚠️ Unknown variable address: ${address}`);
    return false;
  }

  /**
   * Write a variable value
   */
  private writeVariable(address: string, value: boolean): void {
    if (address.startsWith("%IX")) {
      // Inputs are read-only in normal operation
      console.warn(`⚠️ Attempt to write to input: ${address}`);
    } else if (address.startsWith("%QX")) {
      this.outputs.set(address, value);
    } else if (address.startsWith("%MX")) {
      this.markers.set(address, value);
    } else if (address.startsWith("%MW")) {
      // For boolean write to word, use 0/1
      this.memoryWords.set(address, value ? 1 : 0);
    } else {
      console.warn(`⚠️ Unknown variable address: ${address}`);
    }
  }

  /**
   * Write all outputs (to hardware in hardware mode)
   */
  private async writeOutputs(): Promise<void> {
    if (this.mode === "hardware" && this.runtimeClient?.isConnected()) {
      // Write outputs to hardware via RuntimeClient
      for (const [address, value] of this.outputs) {
        try {
          await this.runtimeClient.forceIo(address, value);
          this.forcedAddresses.add(address);
        } catch (error) {
          console.error(`❌ Failed to write ${address}:`, error);
        }
      }
    }
    // In simulation mode, outputs are just stored in memory
  }

  /**
   * Clear all outputs (set to FALSE)
   */
  private async clearAllOutputs(): Promise<void> {
    // Clear simulation outputs
    for (const address of this.outputs.keys()) {
      this.outputs.set(address, false);
    }

    // Unforce hardware outputs
    if (this.mode === "hardware" && this.runtimeClient?.isConnected()) {
      for (const address of this.forcedAddresses) {
        try {
          await this.runtimeClient.unforceIo(address);
        } catch (error) {
          console.error(`❌ Failed to unforce ${address}:`, error);
        }
      }
      this.forcedAddresses.clear();
    }
  }

  /**
   * Set an input value (for simulation/testing)
   */
  setInput(address: string, value: boolean): void {
    if (!address.startsWith("%IX")) {
      console.warn(`⚠️ setInput() called with non-input address: ${address}`);
      return;
    }
    this.inputs.set(address, value);
    console.log(`📥 Input set: ${address} = ${value}`);
  }

  /**
   * Get an output value
   */
  getOutput(address: string): boolean {
    return this.outputs.get(address) ?? false;
  }

  /**
   * Get execution state for visualization
   */
  getExecutionState(): any {
    return {
      scanCount: this.scanCount,
      mode: this.mode,
      inputs: Object.fromEntries(this.inputs),
      outputs: Object.fromEntries(this.outputs),
      markers: Object.fromEntries(this.markers),
      memoryWords: Object.fromEntries(this.memoryWords),
    };
  }

  /**
   * Set state change callback
   */
  setStateChangeCallback(callback: (state: any) => void): void {
    this.onStateChange = callback;
  }

  /**
   * Update program (reload)
   */
  updateProgram(program: LadderProgram): void {
    this.program = program;
    this.initializeVariables();
  }

  /**
   * Cleanup resources
   */
  async cleanup(): Promise<void> {
    await this.stop();
    
    this.inputs.clear();
    this.outputs.clear();
    this.markers.clear();
    this.memoryWords.clear();
  }
}
