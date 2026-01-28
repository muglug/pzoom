<?php
class A {
  /** @var A|null */
  public $a;

  public function __construct() {
    $this->a = rand(0, 1) ? new A : null;
  }
}

function takesA(A $a): void {}

$a = new A();
while ($a) {
  takesA($a);
  $a = $a->a;
};
