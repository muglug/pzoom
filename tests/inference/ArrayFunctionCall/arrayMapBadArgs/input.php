<?php
function foo(int $i) : bool {
  return true;
}

array_map("foo", ["hello"]);
