/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        whatsapp: {
          light: '#efeae2',
          dark: '#111b21',
          header: '#f0f2f5',
          headerDark: '#202c33',
          outgoing: '#d9fdd3',
          outgoingDark: '#005c4b',
          incoming: '#ffffff',
          incomingDark: '#202c33',
          primary: '#008069',
          sidebar: '#ffffff',
          sidebarDark: '#111b21',
          border: '#d1d7db',
          borderDark: '#222e35'
        }
      }
    },
  },
  plugins: [],
}
