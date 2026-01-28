<?php
/**
 * @template TValue
 * @template TArray of non-empty-array<TValue>
 * @param TArray $arr
 * @return TValue
 */
function toList(array $arr): array {
    return reset($arr);
}