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
 * @template T2
 * @template-extends Container<T1>
 */
class ObjectContainer extends Container {}
