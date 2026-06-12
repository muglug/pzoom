<?php
/** @param list<string> $items */
function f(array $items): array {
    $keys = array_reduce(
        $items,
        /**
         * @param list<string> $carry
         * @return list<string>
         */
        static fn(array $carry, string $item): array => array_merge($carry, [$item]),
        [],
    );
    return array_combine($keys, $keys);
}
