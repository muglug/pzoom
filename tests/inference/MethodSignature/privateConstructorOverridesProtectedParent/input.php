<?php
abstract class Channel {
    protected function __construct(protected int $fd) {}
}

final class ForkChannel extends Channel {
    private function __construct(int $fd, private string $label) {
        parent::__construct($fd);
    }

    public static function create(int $fd): self {
        return new self($fd, 'fork');
    }

    public function label(): string {
        return $this->label;
    }
}
