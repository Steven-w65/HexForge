// The graphics boundary is asynchronous. Declare its contract explicitly rather
// than asking Vue Test Utils to infer props by eagerly loading the real renderer.
export const hexCanvasStub = {
  name: 'HexCanvas',
  props: ['fileSize', 'sourceIdentity', 'sourceKey', 'sourceRevision', 'page', 'bytesPerRow', 'selection', 'matches',
    'modifiedOverview', 'templateFields', 'matchLength', 'templateRange', 'editMode', 'theme', 'navigateOffset', 'minimapSettings', 'editDelta'],
  template: '<div data-testid="canvas-stub" />',
}
