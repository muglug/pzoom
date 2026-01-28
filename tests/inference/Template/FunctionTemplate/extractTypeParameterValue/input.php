<?php
/**
 * @template T
 */
interface Type {}

/**
 * @implements Type<int>
 */
final readonly class IntType implements Type {}

/**
 * @template T
 * @implements Type<list<T>>
 */
final readonly class ListType implements Type
{
    /**
     * @param Type<T> $type
     */
    public function __construct(
        public Type $type,
    ) {
    }
}

/**
 * @template T
 * @param Type<T> $type
 * @return T
 */
function extractType(Type $type): mixed
{
    throw new \RuntimeException("Should never be called at runtime");
}

/**
 * @template T
 * @param Type<T> $t
 * @return ListType<T>
 */
function listType(Type $t): ListType
{
    return new ListType($t);
}

function intType(): IntType
{
    return new IntType();
}

$listType = listType(intType());
$list = extractType($listType);
                
