<?php
class ClassWithToString {
    public function __toString(): string {
        return "";
    }
}

function maybeShow(?string $message): void {
    if ($message !== null) {
        echo $message;
    }
}

maybeShow(new ClassWithToString());
