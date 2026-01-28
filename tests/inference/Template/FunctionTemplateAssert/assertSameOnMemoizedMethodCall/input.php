<?php
function testValidUsername(): void {
    try {
        validateUsername("123");
        throw new Exception("Failed to throw exception for short username");
    } catch (Exception $e) {
        assertSame("a", $e->getMessage());
    }

    try {
        validateUsername("invalid#1");
    } catch (Exception $e) {
        assertSame("b", $e->getMessage());
    }
}

/**
 * @psalm-template ExpectedType
 * @psalm-param ExpectedType $expected
 * @psalm-param mixed $actual
 * @psalm-assert =ExpectedType $actual
 */
function assertSame($expected, $actual): void {
    if ($actual !== $expected) {
        throw new Exception("Bad");
    }
}

function validateUsername(string $username): void {
    if (strlen($username) < 5) {
        throw new Exception("Username must be at least 5 characters long");
    }
}