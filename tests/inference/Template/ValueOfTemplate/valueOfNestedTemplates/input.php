<?php
/**
 * @template TValue
 * @template TArray of array<TValue>
 * @param TArray $array
 * @return list<TValue>
 */
function toList(array $array): array {
    return array_values($array);
}
