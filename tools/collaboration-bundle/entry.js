// Yjs v1 wire protocol, paired with pinned Yrs (32-bit client ids).
export {Doc, Text, UndoManager, applyUpdate, encodeStateAsUpdate, encodeStateVector,
  mergeUpdates, createRelativePositionFromTypeIndex, createAbsolutePositionFromRelativePosition,
  encodeRelativePosition, decodeRelativePosition} from 'yjs';
