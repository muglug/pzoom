<?php
/**
 * @template T
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class Container {
    /** @var T */
    private $t;

    /** @param T $t */
    private function __construct($t) {
        $this->t = $t;
    }

    /**
     * @template U
     * @param U $t
     * @return static<U>
     */
    public static function getContainer($t) {
        return new static($t);
    }

    /**
     * @return T
     */
    public function getValue()
    {
        return $this->t;
    }
}

/**
 * @template T1
 * @template-extends Container<T1>
 */
class AContainer extends Container {}

class A {
    function foo() : void {}
}

$b = AContainer::getContainer(new A());