<?php
/** @template T */
trait ImplementerTrait {
    /** @var T */
    private $value;

    /** @psalm-return T */
    public function getValue() {
        return $this->value;
    }
}

/** @template T */
trait GuideTrait {
    /** @psalm-return T */
    abstract public function getValue();
}

/** @template T */
class Test {
    /** @use ImplementerTrait<T> */
    use ImplementerTrait;

    /** @use GuideTrait<int> */
    use GuideTrait;

    public function __construct() {
        $this->value = 123;
    }
}
