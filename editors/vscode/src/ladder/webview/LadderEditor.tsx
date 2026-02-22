import React, { useState, useRef, useEffect } from "react";
import Konva from "konva";
import { Toolbar } from "./Toolbar";
import type { LadderProgram, Rung as RungType, Contact as ContactType, Coil as CoilType } from "../ladderEngine.types";

declare const vscode: any;

export function LadderEditor() {
  const [program, setProgram] = useState<LadderProgram>({
    rungs: [],
    variables: [],
    metadata: {
      name: "New Ladder Program",
      description: "Ladder logic program"
    }
  });
  
  const [selectedTool, setSelectedTool] = useState<string | null>(null);
  const [selectedMode, setSelectedMode] = useState<"simulation" | "hardware">("simulation");
  const [isExecuting, setIsExecuting] = useState(false);
  const [executionState, setExecutionState] = useState<any>(null); // State from engine
  const [scale, setScale] = useState(1); // Zoom level
  const containerRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<Konva.Stage | null>(null);
  const layerRef = useRef<Konva.Layer | null>(null);

  const STAGE_WIDTH = 1200;
  const STAGE_HEIGHT = 2000; // Increased to fit all rungs
  const RUNG_HEIGHT = 100;
  const LEFT_RAIL_X = 50;
  const RIGHT_RAIL_X = 1100;
  const GRID_SIZE = 20; // Snap to grid size

  // Initialize Konva Stage (only once)
  useEffect(() => {
    if (!containerRef.current) return;

    const stage = new Konva.Stage({
      container: containerRef.current,
      width: STAGE_WIDTH,
      height: STAGE_HEIGHT,
    });

    const layer = new Konva.Layer();
    stage.add(layer);

    // Add zoom functionality with mouse wheel
    stage.on('wheel', (e) => {
      e.evt.preventDefault();
      
      const oldScale = stage.scaleX();
      const pointer = stage.getPointerPosition();
      if (!pointer) return;

      const mousePointTo = {
        x: (pointer.x - stage.x()) / oldScale,
        y: (pointer.y - stage.y()) / oldScale,
      };

      // Calculate new scale
      const direction = e.evt.deltaY > 0 ? -1 : 1;
      const scaleBy = 1.1;
      const newScale = direction > 0 ? oldScale * scaleBy : oldScale / scaleBy;
      
      // Limit zoom between 0.3x and 3x
      const limitedScale = Math.max(0.3, Math.min(3, newScale));

      stage.scale({ x: limitedScale, y: limitedScale });
      setScale(limitedScale);

      const newPos = {
        x: pointer.x - mousePointTo.x * limitedScale,
        y: pointer.y - mousePointTo.y * limitedScale,
      };
      stage.position(newPos);
    });

    stageRef.current = stage;
    layerRef.current = layer;

    return () => {
      stage.destroy();
    };
  }, []);

  // Handle canvas click to add elements
  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    const handleClick = (e: any) => {
      if (!selectedTool || e.target !== stage) return;

      const pos = stage.getPointerPosition();
      if (!pos) return;

      // Snap to grid
      const snappedX = Math.round(pos.x / GRID_SIZE) * GRID_SIZE;
      const snappedY = Math.round(pos.y / GRID_SIZE) * GRID_SIZE;

      // Find which rung this belongs to
      const clickedRungIndex = program.rungs.findIndex(rung => 
        Math.abs(rung.y - snappedY) < RUNG_HEIGHT / 2
      );

      if (clickedRungIndex >= 0) {
        addElement(clickedRungIndex, snappedX, snappedY);
        setSelectedTool(null);
      }
    };

    stage.on('click', handleClick);

    return () => {
      stage.off('click', handleClick);
    };
  }, [selectedTool, program.rungs]);

  const addElement = (rungIndex: number, x: number, y: number) => {
    const elementId = `${selectedTool}_${Date.now()}`;
    const updatedRungs = [...program.rungs];

    if (selectedTool === "contact") {
      const newContact: ContactType = {
        id: elementId,
        type: "contact",
        contactType: "NO",
        variable: "%IX0.0",
        position: { x, y }
      };
      updatedRungs[rungIndex].elements.push(newContact);
    } else if (selectedTool === "coil") {
      const newCoil: CoilType = {
        id: elementId,
        type: "coil",
        coilType: "NORMAL",
        variable: "%QX0.0",
        position: { x, y }
      };
      updatedRungs[rungIndex].elements.push(newCoil);
    }

    setProgram(prev => ({ ...prev, rungs: updatedRungs }));
  };

  // Notify backend that webview is ready
  useEffect(() => {
    console.log("[LadderWebview] Component mounted");
    console.log("[LadderWebview] Sending ready message to backend");
    vscode.postMessage({ type: "ready" });
  }, []);

  // Handle messages from extension
  useEffect(() => {
    console.log("[LadderWebview] Setting up message handler");
    const messageHandler = (event: MessageEvent) => {
      const message = event.data;
      console.log("[LadderWebview] Received message:", message.type, message);
      
      switch (message.type) {
        case "loadProgram":
          console.log("[LadderWebview] Loading program with", message.program.rungs.length, "rungs");
          setProgram(message.program);
          break;
        case "executionStarted":
          setIsExecuting(true);
          break;
        case "executionStopped":
          setIsExecuting(false);
          setExecutionState(null);
          break;
        case "stateUpdate":
          setExecutionState(message.state);
          break;
      }
    };

    window.addEventListener("message", messageHandler);
    return () => window.removeEventListener("message", messageHandler);
  }, []);

  // Redraw canvas when program changes
  useEffect(() => {
    if (!layerRef.current) return;

    const layer = layerRef.current;
    layer.destroyChildren();

    // Draw grid
    for (let i = 0; i < STAGE_WIDTH; i += GRID_SIZE) {
      layer.add(new Konva.Line({
        points: [i, 0, i, STAGE_HEIGHT],
        stroke: '#333',
        strokeWidth: 0.5,
        opacity: 0.3,
      }));
    }
    for (let i = 0; i < STAGE_HEIGHT; i += GRID_SIZE) {
      layer.add(new Konva.Line({
        points: [0, i, STAGE_WIDTH, i],
        stroke: '#333',
        strokeWidth: 0.5,
        opacity: 0.3,
      }));
    }

    // Draw power rails
    layer.add(new Konva.Line({
      points: [LEFT_RAIL_X, 0, LEFT_RAIL_X, STAGE_HEIGHT],
      stroke: '#888',
      strokeWidth: 3,
    }));

    layer.add(new Konva.Line({
      points: [RIGHT_RAIL_X, 0, RIGHT_RAIL_X, STAGE_HEIGHT],
      stroke: '#888',
      strokeWidth: 3,
    }));

    // Draw rungs
    program.rungs.forEach((rung, index) => {
      // Horizontal rung line
      layer.add(new Konva.Line({
        points: [LEFT_RAIL_X, rung.y, RIGHT_RAIL_X, rung.y],
        stroke: '#666',
        strokeWidth: 2,
      }));

      // Rung number
      layer.add(new Konva.Text({
        x: 10,
        y: rung.y - 10,
        text: `${index + 1}`,
        fontSize: 14,
        fill: '#ccc',
      }));

      // Draw elements
      rung.elements.forEach((element, elemIndex) => {
        if (element.type === 'contact') {
          drawContact(layer, element, index, elemIndex);
        } else if (element.type === 'coil') {
          drawCoil(layer, element, index, elemIndex);
        }
      });
    });

    layer.batchDraw();
  }, [program, executionState]); // Added executionState dependency

  const drawContact = (layer: Konva.Layer, element: any, rungIndex: number, elemIndex: number) => {
    const { position, contactType, variable } = element;
    
    // Check if this variable is active in execution state
    const isActive = executionState && (
      executionState.inputs?.[variable] ||
      executionState.outputs?.[variable] ||
      executionState.markers?.[variable]
    );
    const color = isActive ? '#FFEB3B' : '#4CAF50'; // Yellow if active, green if inactive
    
    const group = new Konva.Group({ 
      x: position.x, 
      y: position.y,
      draggable: true,
    });

    // Snap to grid on drag
    group.on('dragmove', function() {
      this.x(Math.round(this.x() / GRID_SIZE) * GRID_SIZE);
      this.y(Math.round(this.y() / GRID_SIZE) * GRID_SIZE);
    });

    // Update position on drag end
    group.on('dragend', function() {
      const newX = this.x();
      const newY = this.y();
      setProgram(prev => {
        const updatedRungs = [...prev.rungs];
        if (updatedRungs[rungIndex] && updatedRungs[rungIndex].elements[elemIndex]) {
          updatedRungs[rungIndex].elements[elemIndex].position = { x: newX, y: newY };
        }
        return { ...prev, rungs: updatedRungs };
      });
    });

    // Highlight on hover
    group.on('mouseenter', function() {
      if (stageRef.current) {
        stageRef.current.container().style.cursor = 'move';
      }
    });

    group.on('mouseleave', function() {
      if (stageRef.current) {
        stageRef.current.container().style.cursor = 'default';
      }
    });

    // Background for selection
    group.add(new Konva.Rect({
      x: -5,
      y: -20,
      width: 50,
      height: 40,
      fill: 'transparent',
    }));

    // Contact horizontal lines (left connection)
    group.add(new Konva.Line({
      points: [-20, 0, 0, 0],
      stroke: color,
      strokeWidth: 3,
    }));

    // Contact horizontal lines (right connection)
    group.add(new Konva.Line({
      points: [40, 0, 60, 0],
      stroke: color,
      strokeWidth: 3,
    }));

    if (contactType === 'NO') {
      // Normally open - two vertical lines with gap
      group.add(new Konva.Line({
        points: [15, -15, 15, 15],
        stroke: color,
        strokeWidth: 3,
      }));
      group.add(new Konva.Line({
        points: [25, -15, 25, 15],
        stroke: color,
        strokeWidth: 3,
      }));
    } else {
      // Normally closed - two vertical lines with horizontal connection
      group.add(new Konva.Line({
        points: [15, -15, 15, 15],
        stroke: color,
        strokeWidth: 3,
      }));
      group.add(new Konva.Line({
        points: [25, -15, 25, 15],
        stroke: color,
        strokeWidth: 3,
      }));
      group.add(new Konva.Line({
        points: [15, -10, 25, -10],
        stroke: color,
        strokeWidth: 2,
      }));
    }

    // Variable label
    group.add(new Konva.Text({
      x: 0,
      y: 20,
      text: variable,
      fontSize: 11,
      fill: color,
      fontStyle: 'bold',
    }));

    layer.add(group);
  };

  const drawCoil = (layer: Konva.Layer, element: any, rungIndex: number, elemIndex: number) => {
    const { position, coilType, variable } = element;
    
    // Check if this output is active in execution state
    const isActive = executionState && (
      executionState.outputs?.[variable] ||
      executionState.markers?.[variable]
    );
    const color = isActive ? '#FF9800' : '#2196F3'; // Orange if active, blue if inactive
    
    const group = new Konva.Group({ 
      x: position.x, 
      y: position.y,
      draggable: true,
    });

    // Snap to grid on drag
    group.on('dragmove', function() {
      this.x(Math.round(this.x() / GRID_SIZE) * GRID_SIZE);
      this.y(Math.round(this.y() / GRID_SIZE) * GRID_SIZE);
    });

    // Update position on drag end
    group.on('dragend', function() {
      const newX = this.x();
      const newY = this.y();
      setProgram(prev => {
        const updatedRungs = [...prev.rungs];
        if (updatedRungs[rungIndex] && updatedRungs[rungIndex].elements[elemIndex]) {
          updatedRungs[rungIndex].elements[elemIndex].position = { x: newX, y: newY };
        }
        return { ...prev, rungs: updatedRungs };
      });
    });

    // Highlight on hover
    group.on('mouseenter', function() {
      if (stageRef.current) {
        stageRef.current.container().style.cursor = 'move';
      }
    });

    group.on('mouseleave', function() {
      if (stageRef.current) {
        stageRef.current.container().style.cursor = 'default';
      }
    });

    // Background for selection
    group.add(new Konva.Rect({
      x: -5,
      y: -20,
      width: 50,
      height: 40,
      fill: 'transparent',
    }));

    // Connection lines (left)
    group.add(new Konva.Line({
      points: [-20, 0, 5, 0],
      stroke: color,
      strokeWidth: 3,
    }));

    // Connection lines (right)
    group.add(new Konva.Line({
      points: [35, 0, 60, 0],
      stroke: color,
      strokeWidth: 3,
    }));

    // Coil circle
    group.add(new Konva.Circle({
      x: 20,
      y: 0,
      radius: 15,
      stroke: color,
      strokeWidth: 3,
    }));

    // Type indicators
    if (coilType === 'SET') {
      group.add(new Konva.Text({
        x: 15,
        y: -7,
        text: 'S',
        fontSize: 14,
        fill: color,
        fontStyle: 'bold',
      }));
    } else if (coilType === 'RESET') {
      group.add(new Konva.Text({
        x: 15,
        y: -7,
        text: 'R',
        fontSize: 14,
        fill: color,
        fontStyle: 'bold',
      }));
    } else if (coilType === 'NEGATED') {
      group.add(new Konva.Line({
        points: [10, -10, 30, 10],
        stroke: color,
        strokeWidth: 2,
      }));
    }

    // Variable label
    group.add(new Konva.Text({
      x: 0,
      y: 20,
      text: variable,
      fontSize: 11,
      fill: color,
      fontStyle: 'bold',
    }));

    layer.add(group);
  };

  const addRung = () => {
    const newRung: RungType = {
      id: `rung_${Date.now()}`,
      y: program.rungs.length * RUNG_HEIGHT + 100,
      elements: [],
      connections: []
    };
    
    setProgram(prev => ({
      ...prev,
      rungs: [...prev.rungs, newRung]
    }));
  };

  const handleRun = () => {
    vscode.postMessage({
      type: selectedMode === "simulation" ? "runSimulation" : "runHardware",
      program
    });
  };

  const handleStop = () => {
    vscode.postMessage({
      type: "stop"
    });
    setIsExecuting(false);
  };

  const handleSave = () => {
    vscode.postMessage({
      type: "save",
      program
    });
  };

  return (
    <div className="ladder-editor">
      <Toolbar
        selectedTool={selectedTool}
        onToolSelect={setSelectedTool}
        selectedMode={selectedMode}
        onModeSelect={setSelectedMode}
        isExecuting={isExecuting}
        onRun={handleRun}
        onStop={handleStop}
        onAddRung={addRung}
        onSave={handleSave}
      />
      
      <div className={`canvas-container ${selectedTool ? 'tool-selected' : ''}`}>
        <div ref={containerRef} />
      </div>

      <div className="status-bar">
        {isExecuting && <span className="execution-indicator">● Executing</span>}
        <span>Mode: {selectedMode}</span>
        <span>Rungs: {program.rungs.length}</span>
        <span>Zoom: {Math.round(scale * 100)}%</span>
        {selectedTool && <span>Selected tool: {selectedTool} (click on canvas to place)</span>}
        <span>Grid: {GRID_SIZE}px</span>
      </div>
    </div>
  );
}
