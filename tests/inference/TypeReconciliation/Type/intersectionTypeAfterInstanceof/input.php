<?php
/**
 * @psalm-consistent-constructor
 */
abstract class A {
  /** @var string|null */
  public $foo;

  public static function getFoo(): void {
    $a = new static();
    if ($a instanceof I) {}
    $a->foo = "bar";
  }
}

interface I {}