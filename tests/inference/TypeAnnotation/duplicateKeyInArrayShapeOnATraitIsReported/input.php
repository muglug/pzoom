<?php

/**
 * @psalm-type Attributes = array{
 *   name: string,
 *   email: string,
 *   email: string,
 * }
 */
trait A {
    /**
     * @return Attributes
     */
    public function getAttributes(): array {
        return [];
    }
}
