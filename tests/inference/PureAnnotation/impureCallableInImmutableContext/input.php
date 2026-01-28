<?php

/**
 * @psalm-immutable
 */
class Either
{
    /**
     * @psalm-param callable $_
     */
    public function fold($_): void
    {
        $_();
    }
}

class Whatever
{
    public function __construct()
    {
        $either = new Either();
        $either->fold(
            function (): void {}
        );
    }
}

new Whatever();
                    
