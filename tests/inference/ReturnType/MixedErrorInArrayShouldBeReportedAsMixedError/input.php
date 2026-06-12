<?php
/**
 * @param mixed $configuration
 * @return array{a?: string, b?: int}
 * @psalm-suppress MixedReturnTypeCoercion
 */
function produceParameters(array $configuration): array
{
    $parameters = [];

    foreach (["a", "b"] as $parameter) {
        /** @psalm-suppress MixedAssignment */
        $parameters[$parameter] = $configuration;
    }

    /** @psalm-suppress MixedReturnTypeCoercion */
    return $parameters;
}
