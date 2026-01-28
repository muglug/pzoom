<?php
declare(strict_types=1);

final class A
{
    public function __toString(): string
    {
        return new SplFileInfo("a");
    }
}
                
