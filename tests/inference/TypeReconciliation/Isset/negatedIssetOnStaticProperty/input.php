<?php
class Singleton {
    private static self $instance;
    public function getInstance(): self {
        if (!isset(self::$instance)) {
            self::$instance = new self();
        }
        return self::$instance;
    }
    private function __construct() {}
}
