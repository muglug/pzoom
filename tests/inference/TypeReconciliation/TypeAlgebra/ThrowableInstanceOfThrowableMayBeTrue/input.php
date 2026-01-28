<?php

class Mapper
{
    /** @param class-string<Throwable> $class */
    final public function map(Throwable $throwable, string $class): ?Throwable
    {
        if (! $throwable instanceof $class) {
            return null;
        }

        return $throwable;
    }
}