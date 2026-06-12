<?php
class A {}
class B extends A {}

/** @return B|A */
function foo() {
  return rand(0, 1) ? new A : new B;
}
