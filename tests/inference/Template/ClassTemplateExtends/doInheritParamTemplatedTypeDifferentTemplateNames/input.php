<?php
/**
 * @template T1
 */
interface I {
  /**
   * @param T1 $t
   */
  public function add($t) : void;
}

/**
 * @template T2
 * @template-implements I<T2>
 */
class C implements I {
  /** @var array<T2> */
  private $t;

  /**
   * @param array<T2> $t
   */
  public function __construct(array $t) {
    $this->t = $t;
  }

  /**
   * @inheritdoc
   */
  public function add($t) : void {
    $this->t[] = $t;
  }
}

/** @param C<string> $c */
function foo(C $c) : void {
    $c->add(new stdClass);
}
