<?php
function foo(int $i, string $s) : bool {
  return true;
}

array_filter([1, 2, 3], "foo");
