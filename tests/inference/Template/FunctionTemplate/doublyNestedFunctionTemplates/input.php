<?php
/**
 * @psalm-template Tk
 * @psalm-template Tv
 *
 * @psalm-param iterable<Tk, Tv>                $iterable
 * @psalm-param (callable(Tk, Tv): bool)|null   $predicate
 *
 * @psalm-return iterable<Tk, Tv>
 */
function filter_with_key(iterable $iterable, ?callable $predicate = null): iterable
{
    return (static function () use ($iterable, $predicate): Generator {
        $predicate = $predicate ??
            /**
             * @psalm-param Tk $_k
             * @psalm-param Tv $v
             *
             * @return bool
             */
            function($_k, $v) { return (bool) $v; };

        foreach ($iterable as $k => $v) {
            if ($predicate($k, $v)) {
                yield $k => $v;
            }
        }
    })();
}