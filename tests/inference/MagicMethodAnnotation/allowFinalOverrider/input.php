<?php
class A {
    /**
     * @return static
     */
    public static function foo()
    {
        return new static();
    }

    final public function __construct() {}
}

/**
 * @method static B foo()
 */
final class B extends A {}
