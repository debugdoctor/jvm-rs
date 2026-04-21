package collections;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;

public class CollectionsDemo {
    public static void main(String[] args) {
        System.out.println("=== Arrays Demo ===");
        int[] numbers = {5, 2, 8, 1, 9, 3};

        System.out.println("Original array: " + Arrays.toString(numbers));
        Arrays.sort(numbers);
        System.out.println("Sorted array: " + Arrays.toString(numbers));

        int idx = Arrays.binarySearch(numbers, 5);
        System.out.println("Index of 5: " + idx);

        int[] copy = Arrays.copyOf(numbers, numbers.length);
        System.out.println("Copied array: " + Arrays.toString(copy));

        int[] filled = new int[5];
        Arrays.fill(filled, 42);
        System.out.println("Filled array: " + Arrays.toString(filled));

        System.out.println("\n=== List Demo ===");
        List<String> list = new ArrayList<>();
        list.add("apple");
        list.add("banana");
        list.add("cherry");
        System.out.println("List: " + list);
        System.out.println("List size: " + list.size());
        System.out.println("Contains 'banana': " + list.contains("banana"));
        System.out.println("Get index 1: " + list.get(1));

        System.out.println("\n=== Collections Demo ===");
        Collections.sort(list);
        System.out.println("Sorted list: " + list);

        Collections.reverse(list);
        System.out.println("Reversed list: " + list);

        Collections.shuffle(list);
        System.out.println("Shuffled list: " + list);

        List<Integer> nums = new ArrayList<>();
        for (int i = 1; i <= 5; i++) nums.add(i);
        System.out.println("Numbers: " + nums);
        System.out.println("Sum: " + nums.stream().mapToInt(Integer::intValue).sum());

        System.out.println("\n=== Set Demo ===");
        Set<String> set = new HashSet<>();
        set.add("red");
        set.add("green");
        set.add("blue");
        set.add("red"); // duplicate
        System.out.println("Set: " + set);
        System.out.println("Set size: " + set.size());

        System.out.println("\n=== Map Demo ===");
        Map<String, Integer> map = new HashMap<>();
        map.put("apple", 5);
        map.put("banana", 3);
        map.put("cherry", 7);
        System.out.println("Map: " + map);
        System.out.println("Get 'banana': " + map.get("banana"));
        System.out.println("Map size: " + map.size());

        System.out.println("\n=== All demos passed! ===");
    }
}