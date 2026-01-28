<?php
/**
 * @psalm-consistent-constructor
 */
abstract class A {
  /** @var string|null */
  public $foo;

  public static function getFoo(): void {
    $a = new static();
    if ($a instanceof B) {}
    elseif ($a instanceof C) {}
    else {}
    takesB($a);
  }
}

class B extends A {}
class C extends A {}

function takesB(B $i): void {}
