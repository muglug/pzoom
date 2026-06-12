<?php
function getMethodName(array $data = []): void {
    if (\array_key_exists("custom_name", $data) && $data["custom_name"] !== null) {
    }
    /** @psalm-check-type-exact $data = array<array-key, mixed> */
}
