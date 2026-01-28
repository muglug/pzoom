<?php
/** @template T */
interface Maybe {}

/**
 * @template T
 * @implements Maybe<T>
 */
class Some implements Maybe {
    /** @var T */
    private $value;

    /** @psalm-param T $value */
    public function __construct($value) {
        $this->value = $value;
    }

    /** @psalm-return T */
    public function extract() { return $this->value; }
}

/**
 * @psalm-return Maybe<int>
 */
function repository(): Maybe {
    return new Some(5);
}

$maybe = repository();

if ($maybe instanceof Some) {
    $anInt = $maybe->extract();
}