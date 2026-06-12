<?php
final class IssueData {
    public string $type = "x";
    public string $message = "y";
}

/**
 * @param array<string, list<IssueData>> $files
 * @return list<string>
 */
function collect(array $files): array {
    $out = [];
    foreach ($files as $_uri => $issues) {
        $descriptions = array_map(
            function (IssueData $issue_data): string {
                return '[' . $issue_data->type . '] ' . $issue_data->message;
            },
            $issues,
        );
        foreach ($descriptions as $d) {
            $out[] = $d;
        }
    }
    return $out;
}
