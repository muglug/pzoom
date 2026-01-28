<?php
class StringableJson implements \Stringable, \JsonSerializable {
    public function jsonSerialize(): array
    {
        return [];
    }
    public function __toString(): string
    {
        return json_encode($this);
    }
}
class C {
    private \Stringable&\JsonSerializable $other;
    public function __construct()
    {
        $this->other = new StringableJson();
    }
}
                
