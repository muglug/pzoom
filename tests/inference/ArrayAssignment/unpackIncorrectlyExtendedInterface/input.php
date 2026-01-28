<?php
/**
 * @template TKey
 * @template TValue of scalar
 * @extends Traversable<TKey, TValue>
 */
interface Foo extends Traversable {}

/**
 * @psalm-suppress MissingTemplateParam
 * @template TKey
 * @extends Foo<TKey>
 */
interface Bar extends Foo {}

/**
 * @param Bar<int> $bar
 * @return list<scalar>
 */
function foobar(Bar $bar): array
{
    $unpacked = [...$bar];
    return $unpacked;
}
                
