/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      boxShadow: {
        panel: "0 18px 60px rgba(2, 6, 23, 0.24)",
      },
    },
  },
  plugins: [],
};
