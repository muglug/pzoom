<?php
interface EntityInterface {}

class SomeEntity implements EntityInterface {}

/**
 * @param array<class-string<EntityInterface>, bool> $arr
 * @return array<class-string<EntityInterface>, bool>
 */
function createForEntity(array $arr)
{
    $arr[SomeEntity::class] = true;

    return $arr;
}
