<?php
/**
 * @template TKey as object
 * @template-implements Operation<TKey>
 */
final class Apply implements Operation
{
    /**
     * @return \Closure(array<TKey>): void
     */
    public function i(): Closure
    {
        return
            /**
             * @psalm-param array<TKey> $collection
             */
            static function (array $collection): void{};
    }
}

/**
 * @template TKey as object
 */
interface Operation
{
    /**
     * @psalm-return \Closure(array<TKey>): void
     */
    public function i(): Closure;
}