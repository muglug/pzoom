<?php
class CliTest {
    /** @var list<string> */
    private array $argv = [];

    public function setUp(): void {
        global $argv;
        $this->argv = $argv;
    }

    /** @return list<string> */
    public function get(): array { return $this->argv; }
}
