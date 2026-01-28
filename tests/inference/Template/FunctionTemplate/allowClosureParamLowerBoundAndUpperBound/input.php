<?php
class Foo {}

/**
 * @template TParam as Foo
 * @psalm-param Closure(TParam): void $func
 * @psalm-return Closure(TParam): TParam
 */
function takesClosure(callable $func): callable {
    return
        /**
         * @psalm-param TParam $value
         * @psalm-return TParam
         */
        function ($value) use ($func) {
            $func($value);
            return $value;
        };
}

$value = takesClosure(function(Foo $foo) : void {})(new Foo());