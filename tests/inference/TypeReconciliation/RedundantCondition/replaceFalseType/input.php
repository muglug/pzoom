<?php
function foo(bool $b) : void {
  if (!$b) {
    $b = true;
  }

  if ($b) {}
}
