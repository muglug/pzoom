<?php

/**
 * @psalm-type Attributes = array{
 *   name: string,
 *   email: string,
 *   email: string,
 * }
 */
interface A {
    /**
     * @return Attributes
     */
    public function getAttributes(): array;
}
