<?php

class B2 {
    public string $x = '';
    public string $y = '';

    /** @param array<int, string> $properties */
    public function unser(array $properties): void
    {
        foreach (['x', 'y'] as $key => $property_name) {
            /** @psalm-suppress PossiblyUndefinedIntArrayOffset */
            $this->$property_name = $properties[$key];
        }
    }
}
