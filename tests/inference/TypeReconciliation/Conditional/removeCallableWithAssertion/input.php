<?php
/**
 * @param mixed $p
 * @psalm-assert !callable $p
 * @throws TypeError
 */
function assertIsNotCallable($p): void { if (!is_callable($p)) throw new TypeError; }

/** @return callable|float */
function f() { return rand(0,1) ? "f" : 1.1; }

$a = f();
assert(!is_callable($a));

$b = f();
assertIsNotCallable($b);

atan($a);
atan($b);