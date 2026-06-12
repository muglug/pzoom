<?php
/**
 * @template T
 * @template U
 * @param callable(T): U $predicate
 * @return callable(iterable<int, T>): iterable<int, U>
 */
function map(callable $predicate): callable {
    return function($iter) use ($predicate): int {
        return 1;
    };
}
