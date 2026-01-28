<?php
/**
 * @param scalar|null $value
 */
function Foo($value = null) : bool {
  if (!$value) {
    return true;
  }
  return false;
}