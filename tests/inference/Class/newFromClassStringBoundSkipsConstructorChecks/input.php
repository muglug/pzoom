<?php
interface Task9 {}
class ScanTask9 implements Task9 {
    public function __construct(public string $file) {}
}
/**
 * @param class-string<Task9> $main_task
 * @param list<string> $files
 */
function run(string $main_task, array $files): void {
    foreach ($files as $file) {
        $t = new $main_task($file);
        echo get_class($t);
    }
}
