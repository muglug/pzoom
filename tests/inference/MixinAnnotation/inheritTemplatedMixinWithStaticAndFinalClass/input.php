<?php
/**
 * @template T
 */
class Mixin {
    /**
     * @psalm-var T
     */
    private $var;

    /**
     * @psalm-param T $var
     */
    public function __construct ($var) {
        $this->var = $var;
    }

    /**
     * @psalm-return self<T>
     */
    public function getMixin() {
        return $this;
    }
}

/**
 * @template T as object
 * @mixin Mixin<T>
 */
abstract class Foo {
    /** @var Mixin<T> */
    public object $obj;

    public function __call(string $name, array $args) {
        return $this->obj->$name(...$args);
    }
}

/**
 * @extends Foo<static>
 */
abstract class FooChild extends Foo{}

/**
 * @psalm-suppress MissingConstructor
 */
final class FooGrandChild extends FooChild {}

/**
* @psalm-return Mixin<FooGrandChild>
*/
function test() : Mixin {
    return (new FooGrandChild)->getMixin();
}
