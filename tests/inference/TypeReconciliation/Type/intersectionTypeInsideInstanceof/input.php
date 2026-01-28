<?php
/**
 * @psalm-consistent-constructor
 */
abstract class A {
  /** @var string|null */
  public $foo;

  public static function getFoo(): void {
    $a = new static();
    if ($a instanceof I) {
      takesI($a);
      takesA($a);
    }
  }
}

interface I {}

function takesI(I $i): void {}
function takesA(A $i): void {}