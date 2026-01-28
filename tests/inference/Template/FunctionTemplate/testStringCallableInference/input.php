<?php
class A {
    public static function dup(string $a): string {
        return $a . $a;
    }
}

/**
 * @template T
 * @param iterable<T> $iter
 * @return list<T>
 */
function toArray(iterable $iter): array {
    $data = [];
    foreach ($iter as $val) {
        $data[] = $val;
    }
    return $data;
}

/**
 * @template T
 * @template U
 * @param callable(T): U $predicate
 * @return callable(iterable<int, T>): iterable<int, U>
 */
function map(callable $predicate): callable {
    return
    /** @param iterable<int, T> $iter */
    function(iterable $iter) use ($predicate): iterable {
        foreach ($iter as $key => $value) {
            yield $key => $predicate($value);
        }
    };
}

/** @param list<string> $strings */
function _test(array $strings): void {}
$a =  map([A::class, "dup"])(["a", "b", "c"]);