<?php
class A {
  public function bat() : void {}
}
interface I {
  public function baz() : void;
}

function foo(I $i) : void {
  if ($i instanceof A) {
    $i->bat();
    $i->baz();
  }
}

function bar(A $a) : void {
  if ($a instanceof I) {
    $a->bat();
    $a->baz();
  }
}

class B extends A implements I {
  public function baz() : void {}
}

foo(new B);
bar(new B);