<?php

/**
 * @psalm-suppress MixedReturnTypeCoercion
 * @return list<string>
 */
function foo(){
    /**
     * @var list<mixed>
     * @psalm-suppress MixedReturnTypeCoercion
     */
    return [];
}
