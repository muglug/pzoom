<?php
/**
 * @template T
 * @psalm-consistent-constructor
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
     * @return static
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
