<?php
class A {
  /** @var ?int */
  public $bar;
}

function foo(): ?A {
  return rand(0, 1) ? new A : null;
}

while ($a = foo()) {
  if ($a->bar !== null) {}
}
