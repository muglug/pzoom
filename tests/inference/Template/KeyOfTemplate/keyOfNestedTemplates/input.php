<?php
/**
 * @template TKey of int
 * @template TArray of array<TKey, bool>
 * @param TArray $array
 * @return list<TKey>
 */
function toListOfKeys(array $array): array {
    return array_keys($array);
}
