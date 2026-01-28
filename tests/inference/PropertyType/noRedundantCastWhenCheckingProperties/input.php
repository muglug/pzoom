<?php
class Foo
{
    public array $map;

    public function __construct()
    {
        $this->map = [];
        $this->map["test"] = "test";

        $this->useMap();
    }

    public function useMap(): void
    {
        $keys = array_keys($this->map);
        $key = reset($keys);
        echo (string) $key;
    }
}
