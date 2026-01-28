<?php
/**
 * @template TReturn
 * @param callable():(\Generator<mixed, mixed, mixed, TReturn>|TReturn) $gen
 * @return array<int, TReturn>
 */
function call(callable $gen) : array {
    $return = $gen();
    if ($return instanceof Generator) {
        return [$return->getReturn()];
    }
    /** @var array<int, TReturn> */
    $wrapped_gen = [$gen];
    return $wrapped_gen;
}

$arr = call(
    function() {
        yield 1;
        return "hello";
    }
);