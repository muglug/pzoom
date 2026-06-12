<?php
/**
 * @template-covariant T
 * @template TParent
 */
class Container {}

/**
 * @template T of object
 * @param class-string<T> $class
 * @return Container<covariant T, covariant string>
 */
function wrap(string $class): Container
{
    /** @var Container<covariant T, covariant string> */
    return new Container();
}
