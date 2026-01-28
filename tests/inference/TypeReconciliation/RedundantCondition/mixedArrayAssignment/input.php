<?php
/** @param mixed $arr */
function foo($arr): void {
 if ($arr["a"] === false) {
    /** @psalm-suppress MixedArrayAssignment */
    $arr["a"] = (bool) rand(0, 1);
    if ($arr["a"] === false) {}
  }
}