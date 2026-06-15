<?php
/**
 * @psalm-suppress MixedAssignment
 * @psalm-suppress MixedArrayAssignment
 * @param mixed[] $line
 */
function _renderCells(array $line): void {
  foreach ($line as $cell) {
    $cellOptions = [];
    if (is_array($cell)) {
      $cellOptions = $cell[1];
    }
    if (isset($cellOptions[0])) {
      $cellOptions[0] = $cellOptions[0] . "b";
    } else {
      $cellOptions[0] = "b";
    }
  }
}
