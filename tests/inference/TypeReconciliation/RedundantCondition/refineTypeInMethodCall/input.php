<?php
class A {}

/** @return ?A */
function getA() {
  return rand(0, 1) ? new A : null;
}

function takesA(A $a): void {}

$a = getA();
if ($a instanceof A) {}
/** @psalm-suppress PossiblyNullArgument */
takesA($a);
if ($a instanceof A) {}
