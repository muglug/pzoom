<?php
function foo(bool $b) : void {
  if ($b) {
    $b = false;
  }

  if ($b) {}
}
