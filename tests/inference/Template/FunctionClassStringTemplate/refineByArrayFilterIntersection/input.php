<?php
/**
 * @template T
 * @param array<Bar> $bars
 * @psalm-param class-string<T> $class
 * @return array<T&Bar>
 */
function getBarsThatAreInstancesOf(array $bars, string $class): array
{
    return \array_filter(
        $bars,
        function (Bar $bar) use ($class): bool {
            return $bar instanceof $class;
        }
    );
}

interface Bar {}