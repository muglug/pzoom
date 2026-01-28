<?php
/**
 * @template T of array{
 *    a: string,
 *    b: int
 * }
 */
class MyContainer {
    /** @var T */
    private $value;
    /** @param T $value */
    public function __construct($value) {
        $this->value = $value;
    }
    /** @return T */
    public function getValue() {
      return $this->value;
    }
}
