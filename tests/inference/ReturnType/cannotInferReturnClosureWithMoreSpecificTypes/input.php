<?php
/**
 * @template T
 * @template U
 * @param callable(T): U $predicate
 * @return callable(iterable<int, T>): iterable<int, U>
 */
function map(callable $predicate): callable {
    return
    /** @param iterable<int, int> $iter */
    function($iter) use ($predicate) {
        foreach ($iter as $key => $value) {
            yield $key => $predicate($value);
        }
    };
}

$res = map(function(int $i): string { return (string) $i; })([1,2,3]);
