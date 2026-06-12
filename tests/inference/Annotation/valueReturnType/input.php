<?php
/**
 * @param "a"|"b" $_p
 */
function acceptsLiteral($_p): void {}

/**
 * @return "a"|"b"
 */
function returnsLiteral(): string {
    return rand(0,1) ? "a" : "b";
}

acceptsLiteral(returnsLiteral());
