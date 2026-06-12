<?php
class U {
    public int $a = 0;
    public int $b = 0;
    /** @param array<string, int> $properties */
    public function __construct(array $properties) {
        foreach ($properties as $key => $value) {
            $this->{$key} = $value;
        }
    }
}
