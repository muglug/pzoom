<?php
/**
 * @template T
 */
interface I {
  /**
   * @param T $t
   */
  public function add($t) : void;
}

/**
 * @template T
 *
 * @psalm-suppress MissingTemplateParam
 */
class C implements I {
  /** @var array<T> */
  private $t;

  /**
   * @param array<T> $t
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
