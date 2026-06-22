<?php

// Spreading a list<string> into a parameter that expects a narrower string
// subtype (lowercase-string) is an ArgumentTypeCoercion, exactly as for a
// direct argument. The spread's element value type is checked against each
// parameter it can fill.

final class MethodId
{
    /** @param lowercase-string $method */
    public function __construct(public readonly string $class, public readonly string $method) {}
}

function make(string $symbol): MethodId
{
    return new MethodId(...explode('::', $symbol));
}

echo make('Foo::bar')->class;
