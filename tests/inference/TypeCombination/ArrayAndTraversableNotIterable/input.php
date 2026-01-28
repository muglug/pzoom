<?php declare(strict_types=1);

/** @param mixed $identifier */
function isNullIdentifier($identifier): bool
{
    if ($identifier instanceof \Traversable || is_array($identifier)) {
        expectsTraversableOrArray($identifier);
    }

    return false;
}

/** @param Traversable|array<array-key, mixed> $_a */
function expectsTraversableOrArray($_a): void
{

}
                    
