<?php
/**
 * @psalm-param array{inner:string} $options
 */
function go(array $options): void {
    if (!array_key_exists('size', $options)) {
        throw new Exception('bad');
    }

    /** @psalm-suppress MixedArgument */
    echo $options['\'size\''];
}
