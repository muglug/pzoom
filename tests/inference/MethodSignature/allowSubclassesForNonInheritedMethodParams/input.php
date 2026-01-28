<?php
class A {}
class B extends A {
  public function bar(): void {}
}
class C extends A {
  public function bar(): void {}
}

/** @param B|C $a */
function foo(A $a): void {
  $a->bar();
}
