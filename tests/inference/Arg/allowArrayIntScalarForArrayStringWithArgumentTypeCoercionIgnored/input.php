<?php
/** @param array<array-key> $arr */
function foo(array $arr) : void {
}

/** @return array<int, scalar> */
function bar() : array {
  return [];
}

/** @psalm-suppress ArgumentTypeCoercion */
foo(bar());
