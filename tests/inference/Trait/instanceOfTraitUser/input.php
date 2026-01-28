<?php
trait T {
  public function f(): void {
    if ($this instanceof A) { }
  }
}

class A {
  use T;
}

class B {
  use T;
}
