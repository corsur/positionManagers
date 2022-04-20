export function generateRangeArray(begin, end) {
    if (begin >= end) return [];
    return [...Array(end - begin).keys()].map((num) => num + begin);
}