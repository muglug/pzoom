<?php
/** @psalm-no-seal-methods */
class A {
  public function __call(string $method, array $args) {}
}

class B {}

$a = rand(0, 1) ? new A : new B;

$a->maybeUndefinedMethod();
