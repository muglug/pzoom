<?php
class A {}
class B extends A {
  public function foo(): void {}
}

function takesA(A $a): void {
  if (get_class($a) !== B::class) {
    // do nothing
  } else {
    $a->foo();
  }
}