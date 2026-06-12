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

class Test {
    /** @use ImplementerTrait<int> */
    use ImplementerTrait;

    /** @use GuideTrait<int> */
    use GuideTrait;

    public function __construct() {
        $this->value = 123;
    }
}
