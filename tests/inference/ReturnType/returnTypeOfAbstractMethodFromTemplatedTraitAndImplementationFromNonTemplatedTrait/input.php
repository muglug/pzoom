<?php
trait ImplementerTrait {
    /** @var int */
    private $value;

    public function getValue(): int {
        return $this->value;
    }
}

/** @psalm-template T */
trait GuideTrait {
    /** @psalm-return T */
    abstract public function getValue();
}

class Test {
    use ImplementerTrait;

    /** @template-use GuideTrait<int> */
    use GuideTrait;

    public function __construct() {
        $this->value = 123;
    }
}
