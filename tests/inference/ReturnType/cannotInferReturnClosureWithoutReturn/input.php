<?php
/**
 * @template T
 * @template U
 * @param callable(T): U $predicate
 * @return callable(iterable<int, T>): iterable<int, U>
 */
function map(callable $predicate): callable {
    $a = function($iter) use ($predicate) {
        foreach ($iter as $key => $value) {
            yield $key => $predicate($value);
        }
    };
    return $a;
}

$res = map(function(int $i): string { return (string) $i; })([1,2,3]);
